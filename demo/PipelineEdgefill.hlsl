#include "ConstantBuffer.hlsl"
#include "VertexInput.hlsl"
#include "Utils.hlsl"

struct VS_OUTPUT
{
	float4 Pos : SV_POSITION;
	float2 TexCoord : TEXCOORD0;
};

VS_OUTPUT vs_main( VS_INPUT In )
{
	VS_OUTPUT Out;

	// The "position" output is the coordinates in surface space,
	// So here we remap from [0,1] to [-1,+1]
	Out.Pos.x = (In.Pos.x - 0.5f) * 2.0f;
	Out.Pos.y = (0.5f - In.Pos.y) * 2.0f;
	Out.Pos.z = 1.0f;
	Out.Pos.w = 1.0f;

	Out.TexCoord = In.TexCoord.xy;

	return Out;
}

Texture2D<float4> texEdgefill : register(t0);
Texture2D<float4> texSurfaceFromObject0 : register(t1);
Texture2D<float4> texSurfaceFromObject1 : register(t2);
Texture2D<float4> texSurfaceFromObject2 : register(t3);
Texture2D<float4> texSurfaceFromObject3 : register(t4);
SamplerState smp : register (s0);
SamplerState smpPoint : register (s1);

float4x4 ReadSurfaceFromObject ( float2 texCoords )
{
	float4 surfaceFromObject0 = texSurfaceFromObject0.SampleLevel ( smp, texCoords, 0.0 );
	float4 surfaceFromObject1 = texSurfaceFromObject1.SampleLevel ( smp, texCoords, 0.0 );
	float4 surfaceFromObject2 = texSurfaceFromObject2.SampleLevel ( smp, texCoords, 0.0 );
    float4 surfaceFromObject3 = texSurfaceFromObject3.SampleLevel ( smp, texCoords, 0.0 );
	float4x4 surfaceFromObject;
    surfaceFromObject._m00 = surfaceFromObject0.x;
    surfaceFromObject._m10 = surfaceFromObject0.y;
	surfaceFromObject._m20 = surfaceFromObject0.z;
    surfaceFromObject._m30 = 0.0f;
    surfaceFromObject._m01 = surfaceFromObject1.x;
	surfaceFromObject._m11 = surfaceFromObject1.y;
	surfaceFromObject._m21 = surfaceFromObject1.z;
    surfaceFromObject._m31 = 0.0f;
	surfaceFromObject._m02 = surfaceFromObject2.x;
	surfaceFromObject._m12 = surfaceFromObject2.y;
	surfaceFromObject._m22 = surfaceFromObject2.z;
    surfaceFromObject._m32 = 0.0f;
    surfaceFromObject._m03 = surfaceFromObject3.x;
    surfaceFromObject._m13 = surfaceFromObject3.y;
    surfaceFromObject._m23 = surfaceFromObject3.z;
	surfaceFromObject._m33 = 1.0f;
	return surfaceFromObject;
}

struct PS_OUTPUT
{
	float4 Rt0 : COLOR0;
	float4 Rt1 : COLOR1;
	float4 Rt2 : COLOR2;
    float4 Rt3 : COLOR3;
};

PS_OUTPUT ps_main( VS_OUTPUT In ) : SV_TARGET
{
	PS_OUTPUT result;

    float distortionRatioU = 1.0f;
    float distortionRatioV = 1.0f;
	float4x4 surfaceFromObject;
	
	if ( DistortionMode == 0 )
    {
		// TODO: write me!
		distortionRatioU = 1.0f;
		distortionRatioV = 1.0f;

        float2 edgefillSrc = texEdgefill.SampleLevel(smpPoint, In.TexCoord.xy, 0.0f).xy;
        surfaceFromObject = ReadSurfaceFromObject(edgefillSrc);

		// Write the results. This performs the edgefill step as well.
		// Setting alpha=1.0 for unused channels just so the ImGui debug windows work better.
		// DistortionMode = 0 needs no special fixup for edgefill, you can just copy the transform directly.
		result.Rt0 = float4 ( surfaceFromObject._m00, surfaceFromObject._m10, surfaceFromObject._m20, distortionRatioU );
		result.Rt1 = float4 ( surfaceFromObject._m01, surfaceFromObject._m11, surfaceFromObject._m21, distortionRatioV );
		result.Rt2 = float4 ( surfaceFromObject._m02, surfaceFromObject._m12, surfaceFromObject._m22, 1.0f );
		result.Rt3 = float4 ( surfaceFromObject._m03, surfaceFromObject._m13, surfaceFromObject._m23, 1.0f );	
    }
	else
    {
		// Measure distortion by sampling in a 5-way pattern and taking deltas.
        float2 edgefillUv[5];
        float4x4 surfaceFromObjects[5];
        float3 objectPos[5];
		float2 edgefillSrcUv[5];
        float uVoffset = 1.0f / SurfaceFromObjectTextureSize;
        edgefillUv[0] = float2(In.TexCoord.x,            In.TexCoord.y           );
        edgefillUv[1] = float2(In.TexCoord.x + uVoffset, In.TexCoord.y           );
        edgefillUv[2] = float2(In.TexCoord.x - uVoffset, In.TexCoord.y           );
        edgefillUv[3] = float2(In.TexCoord.x,            In.TexCoord.y + uVoffset);
        edgefillUv[4] = float2(In.TexCoord.x,            In.TexCoord.y - uVoffset);
        for (int i = 0; i < 5; i++)
        {
            edgefillSrcUv[i] = texEdgefill.SampleLevel(smpPoint, edgefillUv[i], 0.0f).xy;
            surfaceFromObjects[i] = ReadSurfaceFromObject(edgefillSrcUv[i]);
            objectPos[i] = float3(surfaceFromObjects[i]._m03, surfaceFromObjects[i]._m13, surfaceFromObjects[i]._m23);
        }
		surfaceFromObject = surfaceFromObjects[0];
		// Note that we only need the _m03/_m13/_m23 components of surfaceFromObject[1-4].
		// Maybe the compiler will remove the extra reads for us, or maybe we'll need to do it ourselves.
        float3 objectPosDeltaU = objectPos[1] - objectPos[2];
        float3 objectPosDeltaV = objectPos[3] - objectPos[4];
        float3 surfacePosDeltaU = mul(surfaceFromObject, float4(objectPosDeltaU, 0.0f)).xyz;
        float3 surfacePosDeltaV = mul(surfaceFromObject, float4(objectPosDeltaV, 0.0f)).xyz;
		// We now have the deltas in surface position in U and V,
		// computed according to the distortion map.
		// Compare these to what we know the U and V deltas should be.
		// Again notice that we only use a single component of each. Will the compiler do the magic for us?
		// Optionally we could use the full vector and compute the anisotropy and be much cleverer
		// in dealing with non-axis-aligned distortion. But this seems to work fine.
		// Note these values can be negative where the mesh self-intersects, e.g. the inside of "elbows"
        distortionRatioU = surfacePosDeltaU.x / uVoffset;
        distortionRatioV = surfacePosDeltaV.y / uVoffset;
		
		// Write the results and do the edgefill step.
		// Setting alpha=1.0 for unused channels just so the ImGui debug windows work better.

		float2 edgefillUvOffset = In.TexCoord - edgefillSrcUv[0];
		if ( ( edgefillUvOffset.x == 0.0 ) && ( edgefillUvOffset.y == 0.0 ) )
        {
			// No actual edgefill, just a copy from src to dest.
			result.Rt0 = float4 ( surfaceFromObject._m00, surfaceFromObject._m10, surfaceFromObject._m20, distortionRatioU );
			result.Rt1 = float4 ( surfaceFromObject._m01, surfaceFromObject._m11, surfaceFromObject._m21, distortionRatioV );
			result.Rt2 = float4 ( surfaceFromObject._m02, surfaceFromObject._m12, surfaceFromObject._m22, 1.0f );
			result.Rt3 = float4 ( surfaceFromObject._m03, surfaceFromObject._m13, surfaceFromObject._m23, 1.0f );
        }
		else
		{
			// DistortionMode = 1 needs some fixup - we can't just copy the 4x3 to a different place.
			// The last vector is the position in object space of (u,v,0.5) in surface space.
			// So if we change UV, we need to update the corresponding object space position.
		
			// Annoyingly, we need the inverse, though only of the 3x3.
			float4x4 surfaceFromObjectNoPos = surfaceFromObject;
			surfaceFromObjectNoPos._m03 = 0.0f;
			surfaceFromObjectNoPos._m13 = 0.0f;
			surfaceFromObjectNoPos._m23 = 0.0f;
			surfaceFromObjectNoPos._m33 = 1.0f;
			float4x4 objectFromSurface = GetInverse4x4 ( surfaceFromObjectNoPos );
			float3 newObjectPos = objectPos[0] + mul ( objectFromSurface, float4 ( edgefillUvOffset.x, edgefillUvOffset.y, 0.0f, 0.0f ) ).xyz;
		
			result.Rt0 = float4 ( surfaceFromObject._m00, surfaceFromObject._m10, surfaceFromObject._m20, distortionRatioU );
			result.Rt1 = float4 ( surfaceFromObject._m01, surfaceFromObject._m11, surfaceFromObject._m21, distortionRatioV );
			result.Rt2 = float4 ( surfaceFromObject._m02, surfaceFromObject._m12, surfaceFromObject._m22, 1.0f );
			result.Rt3 = float4 ( newObjectPos.x,         newObjectPos.y,         newObjectPos.z,         1.0f );
		}
    }

	return result;
}



