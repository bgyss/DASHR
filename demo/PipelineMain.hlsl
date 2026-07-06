#include "ConstantBuffer.hlsl"
#include "VertexInput.hlsl"
#include "Utils.hlsl"

struct VS_OUTPUT
{
	float4 Pos : SV_POSITION;
	float3 TexCoord : TEXCOORD0;
	float3 StartInObject : TEXCOORD1;
	float3 DirectionInObject : TEXCOORD2;
};

VS_OUTPUT vs_main( VS_INPUT In )
{
	VS_OUTPUT Out;
	float4 posObject       = float4 ( In.Pos,       1.0f );
	float4 normObject      = float4 ( In.Normal,    0.0f );
	float4 tangentObject   = float4 ( In.Tangent,   0.0f );
	float4 bitangentObject = float4 ( In.Bitangent, 0.0f );

	Animate ( posObject,
			  normObject,
			  tangentObject,
			  bitangentObject,
			  In.BoneWeights );

	// And now extrude along the normal.
	// Mesh position is assumed to be where heightfield = 0.5, and typically In.TexCoord.z == 1.0, i.e. full extrusion.
	// We do need to extrude a little bit more to cope with non-linear interpolation artifacts,
	// i.e. the matrices/heightfield interpolate in a curvy way, while the mesh of course is made of flat triangles.
	float extraExtrusion = 0.4f;
	posObject.xyz += normObject.xyz * In.TexCoord.z * ( heightScale + extraExtrusion ) * 0.5f;

	// Note that tangentObject and bitangentObject are not used by this pass.
	// Nor are In.Tangent and In.Biangent. They could be removed from the vertex
	// data. However they are used for the distortion pass, and it is useful
	// to be able to use the same vertex buffer for both.
	// Additionally, if you switch back to conventional rendering,
	// you will also need them to do lighting.

	float4 cameraPos = mul(cameraFromObjectMatrix, posObject);
	float4 projectionPos = mul(projectionFromCameraMatrix, cameraPos);
	Out.Pos = projectionPos;
	Out.TexCoord  = In.TexCoord;

	// The direction vector is the line from the camera to the point.
	// In camera space, the camera is at the origin of course, so...
	float3 directionInCamera = cameraPos.xyz;
	float3 directionInObject = mul(objectFromCameraMatrix, directionInCamera).xyz;
	// Note - the direction should NOT be normalized here, it will be normalized
	// after interpolation in the pixel shader. Normalizing here will lead to
	// incorrect interpolation and "swimming" as the camera gets close.

	Out.StartInObject = posObject.xyz;
	Out.DirectionInObject = directionInObject;

	return Out;
}


Texture2D<float4> texAlbedo : register(t0);
Texture2D<float4> texHeight : register(t1);
Texture2D<float4> texNormal : register(t2);
Texture2D<float4> texSurfaceFromObject0 : register(t3);
Texture2D<float4> texSurfaceFromObject1 : register(t4);
Texture2D<float4> texSurfaceFromObject2 : register(t5);
Texture2D<float3> texTeleportMap : register(t6);
SamplerState smp : register (s0);
SamplerState smpPoint : register (s1);

float4x4 ReadSurfaceFromObject ( float2 texCoords )
{
	float4 surfaceFromObject0 = texSurfaceFromObject0.SampleLevel ( smp, texCoords, 0.0 );
	float4 surfaceFromObject1 = texSurfaceFromObject1.SampleLevel ( smp, texCoords, 0.0 );
	float4 surfaceFromObject2 = texSurfaceFromObject2.SampleLevel ( smp, texCoords, 0.0 );
	float4x4 surfaceFromObject;
	surfaceFromObject._11 = surfaceFromObject0.x;
	surfaceFromObject._12 = surfaceFromObject0.y;
	surfaceFromObject._13 = surfaceFromObject0.z;
	surfaceFromObject._14 = surfaceFromObject0.w;
	surfaceFromObject._21 = surfaceFromObject1.x;
	surfaceFromObject._22 = surfaceFromObject1.y;
	surfaceFromObject._23 = surfaceFromObject1.z;
	surfaceFromObject._24 = surfaceFromObject1.w;
	surfaceFromObject._31 = surfaceFromObject2.x;
	surfaceFromObject._32 = surfaceFromObject2.y;
	surfaceFromObject._33 = surfaceFromObject2.z;
	surfaceFromObject._34 = surfaceFromObject2.w;
	surfaceFromObject._41 = 0.0f;
	surfaceFromObject._42 = 0.0f;
	surfaceFromObject._43 = 0.0f;
	surfaceFromObject._44 = 1.0f;
	return surfaceFromObject;
}

// Return is false if it escaped, true if it hit something.
bool TraceRay ( inout int numSteps,
				inout int numTeleports,
				inout float3 posInSurface,
				inout float3 dirInSurface,
				inout float objectDistance,
				float3 startInObject,
				float3 dirInObject,
				float heightScale,
				float heightOffset,
				float stepSize,
				float stepScale
			  )
{
	numSteps = 0;
	objectDistance = 0.0f;
	float3 posInObject = startInObject;

	float height = 0.0f;
	float deltaHeight = -1.0f;

	// heightScale actually scales towards 0.5.
	float envelopeMin = ( 0.0f - 0.5f ) * heightScale + 0.5f + heightOffset - 0.5f;
	float envelopeMax = ( 1.0f - 0.5f ) * heightScale + 0.5f + heightOffset + 0.5f;

	float prevObjectDistance = objectDistance;

	while(numSteps < 10000) // timeout to stop Windows killing the program if you have a bug.
	{
		float3 prevPosInSurface = posInSurface;
		float3 prevDirInSurface = dirInSurface;
		float3 prevPosInObject = posInObject;
		float prevHeight = height;
		float prevDeltaHeight = deltaHeight;

		// Calculate this fresh every time to avoid accumulating error.
		posInObject = startInObject + dirInObject * objectDistance;

		// Do we need to teleport anywhere?
		// The teleport data is POINT sampled (because it has discontinuitiies, so filtering won't work)
		// and if the z value is >0 then it will be used.
		float3 teleport = texTeleportMap.SampleLevel ( smpPoint, posInSurface.xy, 0.0 );
		bool teleported = false;
		if ( teleport.z > 0.0f )
		{
			// Valid data - let's go!
			posInSurface = teleport;
			numTeleports++;
			teleported = true;
		}

		// We need to get the local surfaceFromObject matrix.
		// However, this is stored in SURFACE space, so we have a chicken-and-egg problem.
		// So we use the last surface position to sample the distortion,
		// and hope this is close enough to get a reasonable answer.
		// As we get closer to actual intersection, we take smaller steps anyway,
		// and hopefully it should all just fix itself that way.
		float4x4 surfaceFromObject = ReadSurfaceFromObject ( posInSurface.xy );

		// We step the ray in object space, but have to convert to
		// surface space to sample the distortion and heightfield.
		posInSurface = mul(surfaceFromObject, float4(posInObject, 1.0f)).xyz;
		dirInSurface = mul(surfaceFromObject, float4(dirInObject, 0.0f)).xyz;

		if (DebugMode==2)
		{
			// Hit immediately.
			return true;
		}

		if ( ( posInSurface.x < 0.0f ) ||
			 ( posInSurface.x > 1.0f ) ||
			 ( posInSurface.y < 0.0f ) ||
			 ( posInSurface.y > 1.0f ) ||
			 ( posInSurface.z < envelopeMin ) ||
			 ( posInSurface.z > envelopeMax ) )
		{
			// Escaped the envelope.
			return false;
		}

		float texelHeight = texHeight.SampleLevel ( smp, posInSurface.xy, 0.0 ).r;
		// heightScale actually scales towards 0.5.
		height = ( texelHeight - 0.5f ) * heightScale + 0.5f + heightOffset;
		deltaHeight = height - posInSurface.z;

		if ( deltaHeight > 0.0f )
		{
			// Hit the surface.
			if ( !teleported ) // interpolation over a teleport doesn't work of course!
			{
				// Interpolate backwards along the ray in surface space to get a more precise intersection.
				// This allows larger steps without looking "lumpy"
				// To do even better you'd do a binary chop of the space to zero in on the exact intersection point.
				float deltaDelta = deltaHeight - prevDeltaHeight;
				if (abs(deltaDelta) > 0.00001f)
				{
					float lambda = deltaHeight / deltaDelta;
					// Wrapping can cause instant deltas in texel height, so clamp how far we can interpolate back.
					lambda = clamp ( lambda, 0.0, 1.0f );
					float omLambda = 1.0f - lambda;
					posInSurface   = (prevPosInSurface   * lambda) + (posInSurface   * omLambda);
					posInObject    = (prevPosInObject    * lambda) + (posInObject    * omLambda);
					objectDistance = (prevObjectDistance * lambda) + (objectDistance * omLambda);
				}
			}
			return true;
		}

		prevObjectDistance = objectDistance;

		float stepSkip = max ( 1.0, -deltaHeight * stepScale );
		objectDistance += stepSize * stepSkip;
		numSteps++;
	}

	// If we got here, we hit the step limit.
	return false;
}

// .x component is the direct light, .y is the indirect light.
float2 DoLighting ( float3 normalInObject )
{
	// This does "wrap round gouraud" which looks prettier and shows more shape than standard clamp(N.L)
	float nDotL = dot ( normalInObject, sunDirInObject );

	// Standard NdotL direct lighting will be shadowed
	float direct = clamp ( nDotL, 0.0f, 1.0f );

	// Indirect lighting will not be shadowed.
	float totalIndirect = IndirectLighting * clamp ( nDotL * 0.5f + 0.5f, 0.0f, 1.0f );

	return float2 ( clamp ( direct - totalIndirect, 0.0f, 1.0f ), totalIndirect );
}

float4 ps_main( VS_OUTPUT In ) : SV_TARGET
{
	float4 result = float4 ( 0.0f, 0.0f, 0.0f, 0.0f );

	float4x4 surfaceFromObject;

	float3 posInSurface = In.TexCoord.xyz;
	float3 dirInSurface;

	if (DebugMode==1)
	{
		return texHeight.SampleLevel ( smp, posInSurface.xy, 0.0f );
	}

	float3 startInObject = In.StartInObject;
	// See note in VS why we normalized after interpolation rather than before.
	float3 dirInObject = normalize ( In.DirectionInObject );

	int numSteps = 0;
	int numTeleports = 0;
	int numShadowSteps = 0;
	int numShadowTeleports = 0;

	float objectDistance = 0.0f;

	bool rayHit = TraceRay ( numSteps, numTeleports,
							 posInSurface, dirInSurface,
							 objectDistance,
							 startInObject, dirInObject,
							 heightScale, heightOffset,
							 stepSize, stepScale );

	float3 posInObject = startInObject + dirInObject * objectDistance;

	if (!rayHit)
	{
		result = float4 ( 0.0f, 0.0f, 0.0f, 0.0f );
	}
	else
	{
		// LightingMode:
		// 0 = none (and show heightfield)
		// 1 = surface normal without heightfield
		// 2 = heightfield deltas.
		// 3 = precomputed normal map.
		// 4 = shadow raytrace

		float3 albedo = texAlbedo.Sample ( smp, posInSurface.xy ).rgb;

		// Extract local tangent-space info.
		float4x4 surfaceFromObject = ReadSurfaceFromObject ( posInSurface.xy );
		// But we need the inverse for lighting :-(
		float4x4 objectFromSurface = GetInverse4x4 ( surfaceFromObject );
		float3 tangentBasisObject   = float3 ( objectFromSurface._11, objectFromSurface._21, objectFromSurface._31 );
		float3 bitangentBasisObject = float3 ( objectFromSurface._12, objectFromSurface._22, objectFromSurface._32 );
		float3 normBasisObject      = float3 ( objectFromSurface._13, objectFromSurface._23, objectFromSurface._33 );

		if ( LightingMode == 0 )
		{
			// No lighting, also shows the heightfield, not albedo.
			result.rgb = texHeight.SampleLevel ( smp, posInSurface.xy, 0.0f ).rgb;
		}
		else if ( DebugMode == 2 )
		{
			// No lighting, shows raw albedo.
			result.rgb = texAlbedo.Sample ( smp, posInSurface.xy ).rgb;
		}
		else
		{
			float3 normalObject;
			if ( LightingMode == 1 )
			{
				normalObject = normalize ( normBasisObject );
			}
			else if ( LightingMode == 2 )
			{
				// Take deltas on the UV map to find the heightfield normal.
				// Tune deltaUVStep this to look tolerable - it should be comparable to the size of a texel on the heightfield.
				float baseHeight = texHeight.SampleLevel ( smp, posInSurface.xy, 0.0f ).r;
				float heightPU = texHeight.SampleLevel ( smp, posInSurface.xy + float2 (  deltaUVStep, 0.0f ), 0.0f ).r;
				float heightNU = texHeight.SampleLevel ( smp, posInSurface.xy + float2 ( -deltaUVStep, 0.0f ), 0.0f ).r;
				float heightPV = texHeight.SampleLevel ( smp, posInSurface.xy + float2 ( 0.0f,  deltaUVStep ), 0.0f ).r;
				float heightNV = texHeight.SampleLevel ( smp, posInSurface.xy + float2 ( 0.0f, -deltaUVStep ), 0.0f ).r;
				float deltaU = heightNU - heightPU;
				float deltaV = heightNV - heightPV;

				// This is somewhat hacky to scale the deltas up to be significant relative to the normal.
				float deltaUVScale = 0.25f * heightNormalsScale / deltaUVStep;

				// Then these are the actual vectors that point along the U & V *heightfield surface* (not the surface space)
				float3 heightfieldTangentObject   = tangentBasisObject   - deltaU * deltaUVScale * normBasisObject;
				float3 heightfieldBitangentObject = bitangentBasisObject - deltaV * deltaUVScale * normBasisObject;
				float3 heightfieldNormalObject = cross ( heightfieldTangentObject, heightfieldBitangentObject );
				normalObject = normalize ( heightfieldNormalObject );
			}
			else
			{
				// Standard normal map.
				float3 normalSurface = texNormal.Sample ( smp, posInSurface.xy, 0.0f ).rgb;
				normalSurface.x = normalSurface.x * heightNormalsScale;
				normalSurface.y = normalSurface.y * heightNormalsScale;
				normalObject = mul ( objectFromSurface, float4 ( normalSurface, 0.0f ) ).xyz;
				normalObject = normalize ( normalObject );
			}

			// Do we want shadows?
			// .x is direct, .y is indirect.
			float2 brightness = DoLighting ( normalObject );
			float directLight = brightness.x;
			float indirectLight = brightness.y;
			if ( LightingMode >= 4 )
			{
				if ( directLight > 0.0f )
				{
					float shadowDistance = 0.0f;
					// A cheap trick to avoid shadow acne is to move the ray origin away from the surface
					// *along its normal* (not towards the sun - that doesn't fix glancing angles)
					float3 startInObject = posInObject + shadowAcneScaler * normalize ( normalObject );
					dirInObject = sunDirInObject;

					float3 posInSurface = ( mul ( surfaceFromObject, float4 ( startInObject, 1.0f ) ) ).xyz;
					float3 dirInSurface = ( mul ( surfaceFromObject, float4 ( dirInObject, 0.0f ) ) ).xyz;

					bool hitShadow = TraceRay ( numShadowSteps, numShadowTeleports,
												posInSurface, dirInSurface,
												shadowDistance,
												startInObject, dirInObject,
												heightScale, heightOffset,
												stepSize, stepScale );
					if ( hitShadow )
					{
						directLight = 0.0f;
					}
				}
			}

			result.rgb = albedo * ( directLight + indirectLight );
		}

		result.w = 1.0f;
	}

	if (DebugMode==3)
	{
		result.r = (float)numShadowSteps * 0.01f;
		result.g = (float)numSteps * 0.01f;
		result.b = (float)numTeleports * 0.1f;
		result.a = 1.0f;
	}
	else if (DebugMode == 4)
	{
		if (rayHit)
		{
			result.rg = posInSurface.xy;
			result.b = 0.0f;
			result.a = 1.0f;
			if ( frac(posInSurface.x * 32.0f) < 0.02f ) { result.r += 0.2f; }
			if ( frac(posInSurface.y * 32.0f) < 0.02f ) { result.g += 0.2f; }
		}
	}

	if ( result.a <= 0.0f )
	{
		// "alpha test" also avoids writing depth.
		discard;
	}

	return result;
}

